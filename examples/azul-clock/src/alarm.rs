//! An alarm: a time of day, a repeat (an RFC 5545 RRULE, the one azul's
//! `DateRepeatPicker` edits - `azul_pim::rrule` makes its dates), a label, a
//! sound and a snooze length - and the instants it rings at.
//!
//! The time is WALL time in a zone (the device's, `chrono::Local`, in the
//! app; any `chrono::TimeZone` here, so the tests pin Berlin and New York).
//! An occurrence is a day of the repeat at that wall time, resolved to an
//! instant the way RFC 5545 (3.3.5) and Temporal's "compatible" do it:
//!
//! - a time that does not exist (the spring-forward gap: 02:30 on the last
//!   Sunday of March in Berlin) is read with the offset in force BEFORE the
//!   gap, so it rings as much later as the gap is long (03:30 CEST);
//! - a time that exists twice (the fall-back overlap: 02:30 on the last
//!   Sunday of October) rings ONCE, at the earlier instant.
//!
//! A one-time alarm (no repeat) rings on its `first` day, which [`Alarm::arm`]
//! sets to the next day its time comes; after ringing it switches itself off.
//! A repeating alarm's `first` day is where the repeat counts from (RRULE's
//! DTSTART); unlike an event's first day it rings only when the repeat names
//! it (a Mon / Wed / Fri alarm made on a Tuesday does not ring that Tuesday).
//!
//! The file of an alarm (`clock/alarms/<uuid>.json`) is this struct as JSON,
//! the day as `"2026-10-03"`.

use azul_pim::rrule::{RepeatEnd, Rule};
use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeZone, Utc,
};
use serde::{Deserialize, Serialize};

use crate::tone::Sound;

/// The snooze length a new alarm gets, in minutes.
pub const DEFAULT_SNOOZE_MINUTES: u32 = 10;
/// The longest snooze, in minutes.
pub const MAX_SNOOZE_MINUTES: u32 = 60;
/// An occurrence the computer slept through rings late up to this many
/// minutes; one missed by more is skipped (it would only confuse).
pub const LATE_RING_MINUTES: i64 = 30;
/// No repeat is followed further than this many 400-day windows.
const MAX_WINDOWS: usize = 30;

fn yes() -> bool {
    true
}

fn default_snooze() -> u32 {
    DEFAULT_SNOOZE_MINUTES
}

/// A day as `"2026-10-03"` in the file.
mod ymd {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(day: &NaiveDate, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&day.format("%Y-%m-%d").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveDate, D::Error> {
        let text = String::deserialize(d)?;
        NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").map_err(serde::de::Error::custom)
    }
}

/// One alarm.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alarm {
    /// The file's name (a UUID).
    pub id: String,
    /// "Gym" (may be empty).
    #[serde(default)]
    pub label: String,
    /// 0 to 23.
    pub hour: u32,
    /// 0 to 59.
    pub minute: u32,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The day the repeat counts from; a one-time alarm's day.
    #[serde(with = "ymd")]
    pub first: NaiveDate,
    /// The repeat as an RRULE value (`FREQ=WEEKLY;BYDAY=MO,WE,FR`); empty =
    /// it rings once.
    #[serde(default)]
    pub repeat: String,
    #[serde(default)]
    pub sound: Sound,
    #[serde(default = "default_snooze")]
    pub snooze_minutes: u32,
    /// Vibrate as well (a phone).
    #[serde(default)]
    pub vibrate: bool,
    /// Snoozed: it rings again at this instant (milliseconds since 1970).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
    /// The last occurrence that rang or was let go (milliseconds since
    /// 1970): it never rings again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rang: Option<i64>,
}

/// Why an alarm rings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingKind {
    /// An occurrence of its time and repeat.
    Occurrence,
    /// The end of a snooze.
    Snooze,
}

/// A ring that is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ring {
    /// The instant it was due at.
    pub at: DateTime<Utc>,
    pub kind: RingKind,
}

/// The instant `ms` milliseconds after 1970 (the epoch for nonsense).
#[must_use]
pub fn instant(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_default()
}

/// The instant a wall time falls on in `tz`: a time in a spring-forward gap
/// is read with the offset before the gap; a time that occurs twice is the
/// earlier instant (RFC 5545 3.3.5).
#[must_use]
pub fn resolve_local<Tz: TimeZone>(tz: &Tz, local: NaiveDateTime) -> DateTime<Utc> {
    let _ = (tz, local);
    DateTime::<Utc>::default()
}

impl Alarm {
    /// A one-time alarm at `hour:minute` on `first`, on, with the default
    /// sound and snooze.
    #[must_use]
    pub fn new(id: &str, hour: u32, minute: u32, first: NaiveDate) -> Alarm {
        Alarm {
            id: id.to_string(),
            label: String::new(),
            hour: hour.min(23),
            minute: minute.min(59),
            enabled: true,
            first,
            repeat: String::new(),
            sound: Sound::default(),
            snooze_minutes: DEFAULT_SNOOZE_MINUTES,
            vibrate: false,
            snoozed_until: None,
            last_rang: None,
        }
    }

    /// The same alarm repeating by `rrule`.
    #[must_use]
    pub fn repeating(mut self, rrule: &str) -> Alarm {
        self.repeat = rrule.trim().to_string();
        self
    }

    /// The same alarm with a label.
    #[must_use]
    pub fn labelled(mut self, label: &str) -> Alarm {
        self.label = label.to_string();
        self
    }

    /// The time of day.
    #[must_use]
    pub fn time(&self) -> NaiveTime {
        NaiveTime::from_hms_opt(self.hour.min(23), self.minute.min(59), 0).unwrap_or_default()
    }

    /// The repeat rule; `None` for a one-time alarm - and for a rule that
    /// does not parse (the alarm then rings once, on its first day).
    #[must_use]
    pub fn rule(&self) -> Option<Rule> {
        None
    }

    /// Whether it repeats.
    #[must_use]
    pub fn repeats(&self) -> bool {
        self.rule().is_some()
    }

    /// Up to `limit` occurrences strictly after `after`, in order (on or
    /// off, snooze aside).
    #[must_use]
    pub fn occurrences_after<Tz: TimeZone>(
        &self,
        after: DateTime<Utc>,
        tz: &Tz,
        limit: usize,
    ) -> Vec<DateTime<Utc>> {
        let _ = (after, tz, limit);
        Vec::new()
    }

    /// The first occurrence strictly after `after`.
    #[must_use]
    pub fn next_after<Tz: TimeZone>(&self, after: DateTime<Utc>, tz: &Tz) -> Option<DateTime<Utc>> {
        self.occurrences_after(after, tz, 1).into_iter().next()
    }

    /// When it rings next after `now`: the snooze's end, else its next
    /// occurrence after `now` that did not ring yet; `None` when it is off or
    /// its repeat is over.
    #[must_use]
    pub fn next_ring<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> Option<DateTime<Utc>> {
        let _ = (now, tz);
        None
    }

    /// The ring due at `now`, if any: a snooze that ended, or the latest
    /// occurrence that came (at most [`LATE_RING_MINUTES`] ago) and did not
    /// ring yet.
    #[must_use]
    pub fn due<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> Option<Ring> {
        let _ = (now, tz);
        None
    }

    /// Switches it on for the next time its time comes: a one-time alarm's
    /// day becomes today when its time is still ahead, else tomorrow; a
    /// repeating alarm keeps its day. A snooze and the memory of the last
    /// ring are dropped.
    pub fn arm<Tz: TimeZone>(&mut self, now: DateTime<Utc>, tz: &Tz) {
        let _ = (now, tz);
    }

    /// The ring `ring` happened (or was let go): it never rings again; a
    /// snooze is over; a one-time alarm switches itself off.
    pub fn rang(&mut self, ring: Ring) {
        let _ = ring;
    }

    /// Snooze: it rings again `minutes` after `now` (1 to
    /// [`MAX_SNOOZE_MINUTES`]).
    pub fn snooze(&mut self, now: DateTime<Utc>, minutes: u32) {
        let _ = (now, minutes);
    }

    /// The repeat for the list: "Once", "Every day", "Weekdays", "Weekends",
    /// "Mon Wed Fri", else what the rule says ("Every 2 weeks on Monday").
    #[must_use]
    pub fn repeat_label(&self) -> String {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use chrono_tz::{America::New_York, Europe::Berlin};

    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn a_time_in_the_spring_forward_gap_rings_as_much_later_as_the_gap_is_long() {
        // Berlin, 29 March 2026: 02:00 CET jumps to 03:00 CEST; 02:30 does not exist.
        let local = day(2026, 3, 29).and_hms_opt(2, 30, 0).unwrap();
        assert_eq!(resolve_local(&Berlin, local), utc(2026, 3, 29, 1, 30), "03:30 CEST");
        let alarm = Alarm::new("a", 2, 30, day(2026, 3, 28)).repeating("FREQ=DAILY");
        assert_eq!(
            alarm.occurrences_after(utc(2026, 3, 28, 12, 0), &Berlin, 2),
            vec![utc(2026, 3, 29, 1, 30), utc(2026, 3, 30, 0, 30)]
        );
    }

    #[test]
    fn a_time_that_occurs_twice_in_the_fall_back_overlap_rings_once_at_the_earlier_instant() {
        // Berlin, 25 October 2026: 03:00 CEST falls back to 02:00 CET; 02:30 happens twice.
        let local = day(2026, 10, 25).and_hms_opt(2, 30, 0).unwrap();
        assert_eq!(resolve_local(&Berlin, local), utc(2026, 10, 25, 0, 30), "02:30 CEST");
        let alarm = Alarm::new("a", 2, 30, day(2026, 10, 24)).repeating("FREQ=DAILY");
        assert_eq!(
            alarm.occurrences_after(utc(2026, 10, 24, 12, 0), &Berlin, 3),
            vec![utc(2026, 10, 25, 0, 30), utc(2026, 10, 26, 1, 30), utc(2026, 10, 27, 1, 30)]
        );
    }

    #[test]
    fn a_daily_alarm_keeps_its_wall_time_across_the_change_of_offset() {
        let alarm = Alarm::new("a", 7, 0, day(2026, 10, 20)).repeating("FREQ=DAILY");
        assert_eq!(
            alarm.occurrences_after(utc(2026, 10, 23, 12, 0), &Berlin, 2),
            vec![utc(2026, 10, 24, 5, 0), utc(2026, 10, 25, 6, 0)],
            "07:00 CEST, then 07:00 CET"
        );
        // New York changes a week later (1 November 2026).
        assert_eq!(
            alarm.occurrences_after(utc(2026, 10, 31, 0, 0), &New_York, 2),
            vec![utc(2026, 10, 31, 11, 0), utc(2026, 11, 1, 12, 0)],
            "07:00 EDT, then 07:00 EST"
        );
    }

    #[test]
    fn a_weekday_alarm_skips_the_weekend() {
        // Made on Friday 2 October 2026, after its time: next is Monday.
        let alarm =
            Alarm::new("a", 7, 15, day(2026, 10, 2)).repeating("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR");
        let now = utc(2026, 10, 2, 6, 0); // Friday 08:00 CEST
        assert_eq!(alarm.next_ring(now, &Berlin), Some(utc(2026, 10, 5, 5, 15)));
        assert_eq!(alarm.repeat_label(), "Weekdays");
    }

    #[test]
    fn a_repeating_alarm_does_not_ring_on_a_first_day_its_repeat_does_not_name() {
        // Mon / Wed / Fri, made on Tuesday 6 October 2026 at 02:00 CEST.
        let alarm = Alarm::new("a", 6, 30, day(2026, 10, 6)).repeating("FREQ=WEEKLY;BYDAY=MO,WE,FR");
        assert_eq!(
            alarm.next_ring(utc(2026, 10, 6, 0, 0), &Berlin),
            Some(utc(2026, 10, 7, 4, 30)),
            "Wednesday, not Tuesday"
        );
        assert_eq!(alarm.repeat_label(), "Mon Wed Fri");
        // A COUNT then counts only the days it names.
        let twice = Alarm::new("b", 9, 0, day(2026, 10, 6)).repeating("FREQ=WEEKLY;BYDAY=MO;COUNT=2");
        assert_eq!(
            twice.occurrences_after(utc(2026, 10, 1, 0, 0), &Berlin, 5),
            vec![utc(2026, 10, 12, 7, 0), utc(2026, 10, 19, 7, 0)]
        );
    }

    #[test]
    fn a_repeat_with_an_end_stops_there() {
        let alarm = Alarm::new("a", 8, 0, day(2026, 10, 5)).repeating("FREQ=DAILY;UNTIL=20261007");
        let all = alarm.occurrences_after(utc(2026, 10, 1, 0, 0), &Berlin, 10);
        assert_eq!(all.len(), 3);
        assert_eq!(all[2], utc(2026, 10, 7, 6, 0));
        assert_eq!(alarm.next_ring(utc(2026, 10, 8, 0, 0), &Berlin), None, "over");
    }

    #[test]
    fn a_one_time_alarm_arms_for_the_next_time_its_time_comes_and_then_switches_off() {
        let mut alarm = Alarm::new("a", 6, 30, day(2026, 1, 1));
        alarm.arm(utc(2026, 10, 3, 8, 0), &Berlin); // 10:00 CEST: today's 06:30 is gone
        assert_eq!(alarm.first, day(2026, 10, 4));
        assert_eq!(alarm.next_ring(utc(2026, 10, 3, 8, 0), &Berlin), Some(utc(2026, 10, 4, 4, 30)));
        alarm.arm(utc(2026, 10, 3, 3, 0), &Berlin); // 05:00 CEST: today's is ahead
        assert_eq!(alarm.first, day(2026, 10, 3));
        let ring = alarm.due(utc(2026, 10, 3, 4, 30), &Berlin).expect("due at 06:30");
        assert_eq!(ring.kind, RingKind::Occurrence);
        alarm.rang(ring);
        assert!(!alarm.enabled, "a one-time alarm switches itself off");
        assert_eq!(alarm.next_ring(utc(2026, 10, 3, 4, 31), &Berlin), None);
        assert_eq!(alarm.repeat_label(), "Once");
    }

    #[test]
    fn a_due_occurrence_rings_once_and_a_long_missed_one_not_at_all() {
        let mut alarm = Alarm::new("a", 7, 0, day(2026, 10, 1)).repeating("FREQ=DAILY");
        assert_eq!(alarm.due(utc(2026, 10, 3, 4, 59), &Berlin), None, "not yet");
        let ring = alarm.due(utc(2026, 10, 3, 5, 0), &Berlin).expect("07:00 CEST");
        assert_eq!(ring.at, utc(2026, 10, 3, 5, 0));
        alarm.rang(ring);
        assert!(alarm.enabled, "a repeating alarm stays on");
        assert_eq!(alarm.due(utc(2026, 10, 3, 5, 1), &Berlin), None, "rang once");
        // The computer slept through Sunday 07:00 and woke at 07:20: it rings late.
        assert!(alarm.due(utc(2026, 10, 4, 5, 20), &Berlin).is_some());
        // Woke at 08:00: an hour late, skipped.
        assert_eq!(alarm.due(utc(2026, 10, 4, 6, 0), &Berlin), None);
        assert_eq!(alarm.repeat_label(), "Every day");
    }

    #[test]
    fn a_snoozed_alarm_rings_again_after_the_snooze_and_then_goes_on_as_before() {
        let mut alarm = Alarm::new("a", 7, 0, day(2026, 10, 1)).repeating("FREQ=DAILY");
        let ring = alarm.due(utc(2026, 10, 3, 5, 0), &Berlin).unwrap();
        alarm.rang(ring);
        alarm.snooze(utc(2026, 10, 3, 5, 1), 10);
        assert_eq!(alarm.next_ring(utc(2026, 10, 3, 5, 2), &Berlin), Some(utc(2026, 10, 3, 5, 11)));
        assert_eq!(alarm.due(utc(2026, 10, 3, 5, 10), &Berlin), None);
        let again = alarm.due(utc(2026, 10, 3, 5, 11), &Berlin).expect("the snooze ended");
        assert_eq!(again.kind, RingKind::Snooze);
        alarm.rang(again);
        assert_eq!(alarm.snoozed_until, None);
        assert_eq!(alarm.next_ring(utc(2026, 10, 3, 5, 12), &Berlin), Some(utc(2026, 10, 4, 5, 0)));
    }

    #[test]
    fn an_alarm_that_is_off_never_rings() {
        let mut alarm = Alarm::new("a", 7, 0, day(2026, 10, 1)).repeating("FREQ=DAILY");
        alarm.enabled = false;
        assert_eq!(alarm.next_ring(utc(2026, 10, 3, 0, 0), &Berlin), None);
        assert_eq!(alarm.due(utc(2026, 10, 3, 5, 0), &Berlin), None);
        // Its occurrences are still known (the list shows them greyed).
        assert_eq!(alarm.next_after(utc(2026, 10, 3, 0, 0), &Berlin), Some(utc(2026, 10, 3, 5, 0)));
    }

    #[test]
    fn the_repeat_reads_as_days_or_as_the_rule_says() {
        let a = |rrule: &str| Alarm::new("a", 7, 0, day(2026, 10, 5)).repeating(rrule).repeat_label();
        assert_eq!(a("FREQ=WEEKLY;BYDAY=SA,SU"), "Weekends");
        assert_eq!(a("FREQ=WEEKLY;BYDAY=TU,TH"), "Tue Thu");
        assert_eq!(a("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR,SA,SU"), "Every day");
        assert_eq!(a("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO"), "Every 2 weeks on Monday");
        assert_eq!(a("FREQ=MONTHLY;BYMONTHDAY=5"), "Monthly on day 5");
        assert_eq!(a("NOT A RULE"), "Once", "an unreadable rule rings once");
    }

    #[test]
    fn the_file_is_the_alarm_as_json_with_the_day_as_text() {
        let mut alarm = Alarm::new("7f3a", 6, 30, day(2026, 10, 3))
            .repeating("FREQ=WEEKLY;BYDAY=MO,WE,FR")
            .labelled("Gym");
        alarm.snoozed_until = Some(1_790_000_000_000);
        let json = serde_json::to_string(&alarm).unwrap();
        assert!(json.contains("\"first\":\"2026-10-03\""), "{json}");
        assert!(!json.contains("last_rang"), "unset fields are left out: {json}");
        assert_eq!(serde_json::from_str::<Alarm>(&json).unwrap(), alarm);
        let minimal: Alarm =
            serde_json::from_str(r#"{"id":"x","hour":9,"minute":5,"first":"2026-10-03"}"#).unwrap();
        assert!(minimal.enabled);
        assert_eq!(minimal.snooze_minutes, DEFAULT_SNOOZE_MINUTES);
        assert_eq!(minimal.repeat, "");
    }
}
