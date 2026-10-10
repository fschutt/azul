//! When AzDrive looks at an Azlin drive's period: at the start, then once a day while it runs,
//! an hour after a look that failed, and at the moment a period becomes due when that comes
//! before the next daily look. What a look tells the owner: a new device, a recovery-key
//! lockdown and how long is left to cancel it. No window.

use azcloud_kit::{period::REDEEM_AHEAD_SECS, token::RECOVERY_MEMBER, Redeemed};
use azul_appkit::l10n::Arg;
use azul_storage::time::iso8601;

use crate::periods::{
    check_every, new_device_text, new_devices, recovery_text, Schedule, DAY_SECS, RETRY_SECS,
};

const NOW: u64 = 1_791_450_000;

fn due(schedule: &Schedule, at: u64) -> Vec<String> {
    schedule.due(["d_1", "d_2"], at)
}

#[test]
fn a_drive_is_looked_at_at_the_start_then_once_a_day_and_an_hour_after_a_failed_look() {
    let mut schedule = Schedule::new(DAY_SECS);
    assert_eq!(due(&schedule, NOW), vec!["d_1", "d_2"], "never looked at: now");
    schedule.looked("d_1", &Redeemed::Nothing, NOW);
    schedule.looked("d_2", &Redeemed::Kept(String::from("no answer")), NOW);
    assert!(due(&schedule, NOW + 1).is_empty());
    assert_eq!(due(&schedule, NOW + RETRY_SECS - 1), Vec::<String>::new());
    assert_eq!(due(&schedule, NOW + RETRY_SECS), vec!["d_2"], "a failed look: an hour later");
    assert_eq!(due(&schedule, NOW + DAY_SECS - 1), vec!["d_2"]);
    assert_eq!(due(&schedule, NOW + DAY_SECS), vec!["d_1", "d_2"], "a day later");
}

#[test]
fn a_period_that_becomes_due_before_the_next_daily_look_is_looked_at_when_it_does() {
    let mut schedule = Schedule::new(DAY_SECS);
    // Due (within a week of its end) in two hours: looked at then, not a day later.
    let until = NOW + REDEEM_AHEAD_SECS + 2 * 3600;
    schedule.looked(
        "d_1",
        &Redeemed::NotDue {
            period_until: Some(until),
        },
        NOW,
    );
    assert!(!due(&schedule, NOW + 2 * 3600 - 1).contains(&String::from("d_1")));
    assert!(due(&schedule, NOW + 2 * 3600).contains(&String::from("d_1")));
    // A month bought: the next look a day later (its new end is weeks away).
    schedule.looked(
        "d_1",
        &Redeemed::Extended {
            count: 1,
            period_until: Some(NOW + 32 * DAY_SECS),
        },
        NOW,
    );
    assert!(!due(&schedule, NOW + DAY_SECS - 1).contains(&String::from("d_1")));
    assert!(due(&schedule, NOW + DAY_SECS).contains(&String::from("d_1")));
}

#[test]
fn a_drive_no_longer_in_the_source_list_is_not_looked_at() {
    let mut schedule = Schedule::new(DAY_SECS);
    schedule.looked("d_9", &Redeemed::Nothing, NOW);
    assert_eq!(schedule.due(["d_1"], NOW + 2 * DAY_SECS), vec!["d_1"]);
}

#[test]
fn the_daily_look_can_be_made_more_often_for_a_test_run() {
    assert_eq!(check_every(None), DAY_SECS);
    assert_eq!(check_every(Some("5")), 5);
    assert_eq!(check_every(Some(" 60 ")), 60);
    for wrong in ["", "0", "soon", "-5"] {
        assert_eq!(check_every(Some(wrong)), DAY_SECS, "{wrong:?}");
    }
    // A failed look is tried again within the (shorter) interval too.
    let mut schedule = Schedule::new(5);
    schedule.looked("d_1", &Redeemed::Kept(String::from("busy")), NOW);
    assert_eq!(schedule.due(["d_1"], NOW + 5), vec!["d_1"]);
}

#[test]
fn a_new_device_is_announced_with_the_drive_and_what_to_do_if_it_was_not_you() {
    let said = new_device_text("Work", "m_laptop");
    assert_eq!(said.key, "azdrive-new-device");
    assert_eq!(said.get("drive"), Some(&Arg::from("Work")));
    assert_eq!(said.get("member"), Some(&Arg::from("m_laptop")));
    let english = crate::l10n::EN;
    assert!(english.contains("azdrive-new-device = A new device was added to \"{ $drive }\""));
    assert!(english.contains("Not you?"));
}

#[test]
fn the_recovery_codes_device_is_announced_as_a_lockdown_not_as_a_new_device() {
    let new = [String::from("m_laptop"), String::from(RECOVERY_MEMBER)];
    assert_eq!(new_devices(&new), vec!["m_laptop"]);
}

#[test]
fn a_recovery_lockdown_says_how_long_the_owner_has_to_cancel_it() {
    let until = NOW + 48 * 3_600;
    let said = recovery_text("Work", until, NOW);
    assert_eq!(said.key, "azdrive-recovery-used");
    assert_eq!(said.get("drive"), Some(&Arg::from("Work")));
    assert_eq!(said.get("hours"), Some(&Arg::Int(48)));
    assert_eq!(said.get("at"), Some(&Arg::from(iso8601(until))));
    let soon = recovery_text("Work", NOW + 90 * 60, NOW);
    assert_eq!(soon.get("hours"), Some(&Arg::Int(2)));
    let now = recovery_text("Work", NOW + 600, NOW);
    assert_eq!(now.get("hours"), Some(&Arg::Int(0)), "less than an hour");
}
