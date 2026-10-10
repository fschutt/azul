//! "Restore as of..." of an Azlin drive: the time typed (a while ago, or a UTC time) within the
//! days the drive keeps, and what the window says after. No window.

use azul_storage::time::iso8601;

use crate::restore::{parse_as_of, restored_text, Restored, DEFAULT_AS_OF, RESTORE_DAYS};

const NOW: u64 = 1_791_450_000;

#[test]
fn a_time_is_a_while_ago_or_a_utc_time_within_the_days_the_drive_keeps() {
    assert_eq!(parse_as_of("1 hour ago", NOW), Ok(NOW - 3_600));
    assert_eq!(parse_as_of(" 2 hours ago ", NOW), Ok(NOW - 7_200));
    assert_eq!(parse_as_of("30 minutes ago", NOW), Ok(NOW - 1_800));
    assert_eq!(parse_as_of("3 days ago", NOW), Ok(NOW - 3 * 86_400));
    assert_eq!(parse_as_of(DEFAULT_AS_OF, NOW), Ok(NOW - 3_600));
    let at = NOW - 5_000;
    assert_eq!(parse_as_of(&iso8601(at), NOW), Ok(at));
    // `YYYY-MM-DD HH:MM`, UTC.
    let minute = at - at % 60;
    let short = iso8601(minute)[..16].replace('T', " ");
    assert_eq!(parse_as_of(&short, NOW), Ok(minute), "{short}");
    for wrong in [
        "",
        "soon",
        "-1 hours ago",
        "2 weeks ago",
        "2026-13-40 99:99",
    ] {
        assert!(parse_as_of(wrong, NOW).is_err(), "{wrong:?}");
    }
    let later = parse_as_of(&iso8601(NOW + 60), NOW).unwrap_err();
    assert!(later.contains("to come"), "{later}");
    let too_old = parse_as_of(&iso8601(NOW - RESTORE_DAYS * 86_400 - 1), NOW).unwrap_err();
    assert!(too_old.contains("14 days"), "{too_old}");
}

#[test]
fn what_a_restore_did_is_said_with_the_drive_and_the_time() {
    let at = NOW - 3_600;
    let text = restored_text("Work", at, &Restored::Files(4));
    assert!(
        text.contains("\"Work\"") && text.contains(&iso8601(at)),
        "{text}"
    );
    assert!(text.contains("4 files"), "{text}");
    let text = restored_text("Work", at, &Restored::Objects(7));
    assert!(text.contains("7 objects"), "{text}");
    let text = restored_text("Work", at, &Restored::Queued(String::from("r_1")));
    assert!(text.contains("r_1") && text.contains("later"), "{text}");
}

/// A while ago typed in German ("vor 2 Stunden"): the dialog's words are the window's language,
/// and so is what the user types.
#[test]
fn a_while_ago_is_typed_in_german_too() {
    assert_eq!(parse_as_of("vor 1 Stunde", NOW), Ok(NOW - 3_600));
    assert_eq!(parse_as_of("vor 2 Stunden", NOW), Ok(NOW - 7_200));
    assert_eq!(parse_as_of("vor 30 Minuten", NOW), Ok(NOW - 1_800));
    assert_eq!(parse_as_of("vor 1 Minute", NOW), Ok(NOW - 60));
    assert_eq!(parse_as_of(" Vor 3 Tagen ", NOW), Ok(NOW - 3 * 86_400));
    assert_eq!(parse_as_of("vor 1 Tag", NOW), Ok(NOW - 86_400));
    for wrong in ["vor 2 Wochen", "vor Stunden", "2 Stunden"] {
        assert!(parse_as_of(wrong, NOW).is_err(), "{wrong:?}");
    }
}
