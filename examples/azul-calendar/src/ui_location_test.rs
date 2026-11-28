use super::*;
use azul_appkit::l10n::set_locale;

fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn a_place_is_said_in_the_windows_language() {
    crate::l10n::in_english();
    assert_eq!(Words.year(2026), "2026");
    assert_eq!(Words.month(11), "November");
    assert_eq!(Words.month(1), "January");
    assert_eq!(Words.day(day(2026, 11, 28)), "Sat 28 Nov");
    assert_eq!(Words.range(day(2026, 11, 23), day(2026, 11, 29)), "23 - 29 Nov");
    assert_eq!(Words.range(day(2026, 11, 30), day(2026, 12, 6)), "30 Nov - 6 Dec");
    assert_eq!(Words.list(), "Next 7 Days");
    set_locale("de-DE");
    assert_eq!(Words.month(3), "März");
    set_locale("en-US");
}

#[test]
fn a_month_that_does_not_exist_is_held_to_the_year() {
    crate::l10n::in_english();
    assert_eq!(Words.month(0), "January");
    assert_eq!(Words.month(13), "December");
}

#[test]
fn every_month_name_has_its_word_in_both_languages() {
    for language in ["en-US", "de-DE"] {
        crate::l10n::in_english();
        set_locale(language);
        for month in 1..=12 {
            let said = Words.month(month);
            assert!(
                !said.is_empty() && !said.starts_with("kit-month-"),
                "{language}: month {month} is {said:?}"
            );
        }
    }
    set_locale("en-US");
}
