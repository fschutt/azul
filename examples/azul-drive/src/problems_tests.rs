//! What AzDrive shows of a drive's errors (C11, D33): the table's text in the user's language
//! with the error ID, in the status line - a transient one only after two minutes - and as a
//! notification for the ones the user must act on, at most once an hour per drive. No window.

use azcloud_kit::{
    user_errors::{Code, Lang},
    UserError,
};
use azul_storage::{DriveError, ServiceError};

use crate::problems::{describe_in, lang_from, Problems, NOTIFY_EVERY_SECS, TRANSIENT_QUIET_SECS};

const NOW: u64 = 1_791_450_000;

fn unpaid() -> DriveError {
    DriveError::Service(ServiceError {
        status: 403,
        code: String::from("AccessDenied"),
        message: String::from("read-only"),
        request_id: Some(String::from("n2-81723")),
        azlin_error: Some(String::from("read_only_unpaid")),
        ..ServiceError::default()
    })
}

fn user(e: &DriveError) -> UserError {
    UserError::from_drive_error(e).unwrap()
}

#[test]
fn the_language_comes_from_the_locale_of_the_environment() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    };
    assert_eq!(lang_from(env(&[("LC_ALL", "de_DE.UTF-8")])), Lang::De);
    assert_eq!(lang_from(env(&[("LC_ALL", ""), ("LANG", "de_AT")])), Lang::De);
    assert_eq!(
        lang_from(env(&[("LC_MESSAGES", "en_GB"), ("LANG", "de_DE")])),
        Lang::En,
        "LC_MESSAGES before LANG"
    );
    assert_eq!(lang_from(env(&[])), Lang::En);
}

#[test]
fn a_storage_error_reads_as_the_tables_text_with_its_error_id_and_a_local_one_as_before() {
    let text = describe_in(&unpaid(), Lang::En);
    assert_eq!(
        text,
        "Your last payment didn't go through. Your files are safe and readable. \
         Error ID: n2-81723"
    );
    assert!(describe_in(&unpaid(), Lang::De).starts_with("Deine letzte Zahlung"));
    let missing = DriveError::NotFound {
        key: String::from("a.txt"),
    };
    assert_eq!(describe_in(&missing, Lang::De), missing.to_string());
}

#[test]
fn a_transient_problem_shows_after_two_minutes_and_one_the_user_must_act_on_at_once() {
    let mut problems = Problems::default();
    let busy = user(&DriveError::Transport(String::from("refused")));
    assert!(!problems.record("d_1", busy.clone(), NOW), "no notification for a transient one");
    assert_eq!(problems.shown("d_1", NOW + TRANSIENT_QUIET_SECS - 1), None);
    // Still failing: it keeps its start.
    assert!(!problems.record("d_1", busy, NOW + 60));
    assert_eq!(
        problems.shown("d_1", NOW + TRANSIENT_QUIET_SECS).map(|p| p.code),
        Some(Code::Network)
    );
    assert!(problems.record("d_2", user(&unpaid()), NOW), "a notification");
    assert_eq!(
        problems.shown("d_2", NOW).map(|p| p.code),
        Some(Code::ReadOnlyUnpaid)
    );
    problems.clear("d_1");
    assert_eq!(problems.shown("d_1", NOW + 600), None, "it works again");
    assert_eq!(problems.shown("d_9", NOW), None);
}

#[test]
fn a_drive_notifies_at_most_once_an_hour() {
    let mut problems = Problems::default();
    assert!(problems.record("d_1", user(&unpaid()), NOW));
    assert!(!problems.record("d_1", user(&unpaid()), NOW + 60));
    problems.clear("d_1");
    assert!(
        !problems.record("d_1", user(&unpaid()), NOW + 120),
        "a new problem within the hour"
    );
    assert!(problems.record("d_1", user(&unpaid()), NOW + NOTIFY_EVERY_SECS));
    assert!(problems.record("d_2", user(&unpaid()), NOW + 60), "another drive");
}
