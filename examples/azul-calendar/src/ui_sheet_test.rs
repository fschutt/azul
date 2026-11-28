use super::*;

#[test]
fn a_delete_says_which_event_it_took_and_never_an_empty_title() {
    crate::l10n::in_english();
    assert_eq!(deleted_text("Review"), "Deleted \"Review\".");
    assert_eq!(deleted_text("  "), "Deleted \"(No title)\".");
}

#[test]
fn a_sheet_is_for_file_or_for_one_occurrence_of_one_event() {
    let day = NaiveDate::from_ymd_opt(2026, 11, 28).unwrap();
    let a = SheetFor::Event { id: String::from("x"), date: day };
    assert_eq!(a, SheetFor::Event { id: String::from("x"), date: day });
    assert_ne!(a, SheetFor::Event { id: String::from("x"), date: day.succ_opt().unwrap() });
    assert_ne!(a, SheetFor::File);
}
