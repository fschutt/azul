//! What AzClock hands to the OS so alarms and timers ring while it is
//! closed: one notification per upcoming instant, posted with a delivery
//! time (`Notification::with_deliver_at`; macOS / iOS keep it as a
//! `UNTimeIntervalNotificationTrigger`, Windows as a scheduled toast, other
//! systems hold it in the running process).
//!
//! The app expands the repeats itself - the OS only ever sees one-shot
//! instants, which every backend can keep, already DST-correct in the
//! device's zone - and tops them up whenever it runs. An alarm takes up to
//! [`ALARM_SLOTS`] slots (its next occurrences within [`HORIZON_DAYS`]),
//! each a fixed id (`azclock-alarm-<uuid>-<slot>`): posting a slot again
//! REPLACES what the OS holds for it, so a changed alarm, a new day or a
//! new zone needs no bookkeeping of what was scheduled before; slots an
//! alarm does not fill are withdrawn. Apple keeps at most 64 pending
//! notifications per app: the plan keeps the soonest [`BUDGET`].
//!
//! The payload (`alarm:<uuid>:<ms>`, `snooze:<uuid>:<ms>`, `timer:<uuid>`)
//! survives the process, so the tap that launches AzClock still names what
//! rang.

use chrono::{DateTime, Duration, TimeZone, Utc};

use crate::{
    alarm::{instant, Alarm},
    fmt,
    timer::CountdownTimer,
};

/// Occurrences one alarm keeps scheduled.
pub const ALARM_SLOTS: usize = 7;
/// How far ahead occurrences are scheduled.
pub const HORIZON_DAYS: i64 = 8;
/// The most notifications scheduled at once (Apple's limit is 64).
pub const BUDGET: usize = 60;

/// What a scheduled notification is about; its text is the notification's
/// payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// An occurrence of an alarm, due at `at_ms`.
    Alarm { id: String, at_ms: i64 },
    /// The end of an alarm's snooze.
    Snooze { id: String, at_ms: i64 },
    /// A timer's end.
    Timer { id: String },
}

impl Payload {
    /// `alarm:<uuid>:<ms>`, `snooze:<uuid>:<ms>`, `timer:<uuid>`.
    #[must_use]
    pub fn to_text(&self) -> String {
        String::new()
    }

    /// The payload a notification came back with; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Payload> {
        let _ = text;
        None
    }

    /// The alarm or timer it is about.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Payload::Alarm { id, .. } | Payload::Snooze { id, .. } | Payload::Timer { id } => id,
        }
    }
}

/// One notification to schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// The notification's id (posting it again replaces it).
    pub id: String,
    /// When it shows (ms since 1970).
    pub at_ms: i64,
    pub title: String,
    pub body: String,
    pub payload: Payload,
    /// The Snooze button's minutes; 0 = no Snooze button (a timer).
    pub snooze_minutes: u32,
    /// No sound (the alarm's sound is Silent).
    pub silent: bool,
}

/// What to post and what to withdraw.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Soonest first.
    pub post: Vec<Planned>,
    pub withdraw: Vec<String>,
}

/// The id of an alarm's `slot`th scheduled occurrence.
#[must_use]
pub fn alarm_slot_id(alarm_id: &str, slot: usize) -> String {
    format!("azclock-alarm-{alarm_id}-{slot}")
}

/// The id of an alarm's snooze.
#[must_use]
pub fn snooze_id(alarm_id: &str) -> String {
    format!("azclock-snooze-{alarm_id}")
}

/// The id of a timer's end.
#[must_use]
pub fn timer_id(timer_id: &str) -> String {
    format!("azclock-timer-{timer_id}")
}

/// Every id an alarm may have scheduled (to withdraw them all).
#[must_use]
pub fn ids_of_alarm(alarm_id: &str) -> Vec<String> {
    let mut ids: Vec<String> = (0..ALARM_SLOTS).map(|k| alarm_slot_id(alarm_id, k)).collect();
    ids.push(snooze_id(alarm_id));
    ids
}

/// The notifications for `alarms` and `timers` at `now`, in `tz`.
#[must_use]
pub fn plan<Tz: TimeZone>(
    alarms: &[Alarm],
    timers: &[CountdownTimer],
    now: DateTime<Utc>,
    tz: &Tz,
    twelve_hour: bool,
) -> Plan {
    let _ = (alarms, timers, now, tz, twelve_hour, instant, Duration::zero(), fmt::clock);
    Plan::default()
}

/// What changed from the plan handed over last (`None`: nothing is known
/// about what the OS holds - everything is posted and withdrawn): the posts
/// that are new or different, and the ids that were posted before and are
/// not any more.
#[must_use]
pub fn diff(previous: Option<&Plan>, next: &Plan) -> Plan {
    let _ = (previous, next);
    Plan::default()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use chrono_tz::Europe::Berlin;

    use super::*;
    use crate::timer::MINUTE_MS;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    fn now() -> DateTime<Utc> {
        utc(2026, 10, 3, 8, 0) // Saturday 10:00 CEST
    }

    #[test]
    fn a_daily_alarm_is_scheduled_for_its_next_seven_days() {
        let alarm = Alarm::new("a1", 7, 0, day(2026, 10, 1)).repeating("FREQ=DAILY").labelled("Wake up");
        let plan = plan(&[alarm], &[], now(), &Berlin, false);
        assert_eq!(plan.post.len(), ALARM_SLOTS);
        assert_eq!(plan.post[0].id, "azclock-alarm-a1-0");
        assert_eq!(plan.post[0].at_ms, utc(2026, 10, 4, 5, 0).timestamp_millis());
        assert_eq!(plan.post[6].at_ms, utc(2026, 10, 10, 5, 0).timestamp_millis());
        assert_eq!(plan.post[0].title, "Wake up");
        assert_eq!(plan.post[0].body, "07:00");
        assert_eq!(plan.post[0].snooze_minutes, 10);
        assert_eq!(
            plan.post[0].payload,
            Payload::Alarm { id: "a1".into(), at_ms: utc(2026, 10, 4, 5, 0).timestamp_millis() }
        );
        assert_eq!(plan.withdraw, vec!["azclock-snooze-a1".to_string()], "no snooze");
    }

    #[test]
    fn a_weekly_alarm_fills_what_the_horizon_holds_and_withdraws_its_other_slots() {
        let alarm = Alarm::new("m", 6, 30, day(2026, 10, 1)).repeating("FREQ=WEEKLY;BYDAY=MO");
        let plan = plan(&[alarm], &[], now(), &Berlin, true);
        assert_eq!(plan.post.len(), 1, "one Monday in the next eight days");
        assert_eq!(plan.post[0].at_ms, utc(2026, 10, 5, 4, 30).timestamp_millis());
        assert_eq!(plan.post[0].title, "Alarm", "no label");
        assert_eq!(plan.post[0].body, "6:30 AM");
        for k in 1..ALARM_SLOTS {
            assert!(plan.withdraw.contains(&alarm_slot_id("m", k)), "slot {k}");
        }
    }

    #[test]
    fn an_alarm_that_is_off_withdraws_every_id_it_may_have() {
        let mut alarm = Alarm::new("off", 7, 0, day(2026, 10, 1)).repeating("FREQ=DAILY");
        alarm.enabled = false;
        let plan = plan(&[alarm], &[], now(), &Berlin, false);
        assert!(plan.post.is_empty());
        assert_eq!(plan.withdraw, ids_of_alarm("off"));
    }

    #[test]
    fn a_snoozed_alarm_schedules_the_end_of_its_snooze() {
        let mut alarm = Alarm::new("s", 9, 50, day(2026, 10, 3));
        alarm.snooze(now(), 10);
        let plan = plan(&[alarm], &[], now(), &Berlin, false);
        let snooze = plan.post.iter().find(|p| p.id == snooze_id("s")).expect("the snooze");
        assert_eq!(snooze.at_ms, now().timestamp_millis() + 10 * MINUTE_MS);
        assert!(matches!(snooze.payload, Payload::Snooze { .. }));
    }

    #[test]
    fn a_running_timer_is_scheduled_at_its_end_and_a_paused_one_withdrawn() {
        let t0 = now().timestamp_millis();
        let mut tea = CountdownTimer::new("tea", "Tea", 10 * MINUTE_MS);
        tea.start(t0);
        let mut pasta = CountdownTimer::new("pasta", "", 12 * MINUTE_MS);
        pasta.start(t0);
        pasta.pause(t0 + MINUTE_MS);
        let plan = plan(&[], &[tea, pasta], now(), &Berlin, false);
        assert_eq!(plan.post.len(), 1);
        assert_eq!(plan.post[0].id, "azclock-timer-tea");
        assert_eq!(plan.post[0].at_ms, t0 + 10 * MINUTE_MS);
        assert_eq!(plan.post[0].title, "Tea");
        assert_eq!(plan.post[0].body, "Time is up (10:00)");
        assert_eq!(plan.post[0].snooze_minutes, 0, "a timer has no snooze");
        assert_eq!(plan.withdraw, vec!["azclock-timer-pasta".to_string()]);
    }

    #[test]
    fn the_budget_keeps_the_soonest_and_withdraws_the_rest() {
        let alarms: Vec<Alarm> = (0..10)
            .map(|i| Alarm::new(&format!("a{i}"), 6 + i, 0, day(2026, 10, 1)).repeating("FREQ=DAILY"))
            .collect();
        let plan = plan(&alarms, &[], now(), &Berlin, false);
        assert_eq!(plan.post.len(), BUDGET);
        assert!(plan.post.windows(2).all(|w| w[0].at_ms <= w[1].at_ms), "soonest first");
        let dropped = 10 * ALARM_SLOTS - BUDGET;
        let slot_withdrawals = plan.withdraw.iter().filter(|id| id.starts_with("azclock-alarm-")).count();
        assert_eq!(slot_withdrawals, dropped);
        let last_kept = plan.post.last().unwrap().at_ms;
        assert!(last_kept <= utc(2026, 10, 10, 6, 0).timestamp_millis());
    }

    #[test]
    fn the_payload_names_what_rang_and_reads_back() {
        let p = Payload::Alarm { id: "7f3a-11".into(), at_ms: 1_790_000_000_000 };
        assert_eq!(p.to_text(), "alarm:7f3a-11:1790000000000");
        assert_eq!(Payload::parse(&p.to_text()), Some(p));
        let s = Payload::Snooze { id: "x".into(), at_ms: 5 };
        assert_eq!(Payload::parse(&s.to_text()), Some(s));
        let t = Payload::Timer { id: "tea".into() };
        assert_eq!(t.to_text(), "timer:tea");
        assert_eq!(Payload::parse("timer:tea"), Some(t));
        assert_eq!(Payload::parse("alarm:x:notanumber"), None);
        assert_eq!(Payload::parse("aztasks-123"), None);
        assert_eq!(Payload::parse(""), None);
    }

    #[test]
    fn only_what_changed_is_handed_over_again() {
        let alarm = Alarm::new("a1", 7, 0, day(2026, 10, 1)).repeating("FREQ=DAILY");
        let other = Alarm::new("a2", 8, 0, day(2026, 10, 1)).repeating("FREQ=DAILY");
        let first = plan(&[alarm.clone(), other], &[], now(), &Berlin, false);
        assert_eq!(diff(None, &first), first, "nothing known: everything");
        let same = plan(&[alarm.clone(), Alarm::new("a2", 8, 0, day(2026, 10, 1)).repeating("FREQ=DAILY")], &[], now(), &Berlin, false);
        let d = diff(Some(&first), &same);
        assert!(d.post.is_empty() && d.withdraw.is_empty(), "{d:?}");
        // a2 was deleted: its slots go; a1 is untouched.
        let after = plan(&[alarm], &[], now(), &Berlin, false);
        let d = diff(Some(&first), &after);
        assert!(d.post.is_empty());
        assert_eq!(d.withdraw.len(), ALARM_SLOTS);
        assert!(d.withdraw.iter().all(|id| id.starts_with("azclock-alarm-a2-")));
    }
}
