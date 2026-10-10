//! Switching the language changes the words (feature `azul`: azul's Fluent): a known label of
//! appkit's resources in English, in German, and English for a language without words of its
//! own.

use crate::l10n::{
    app_word, date_text, decimal, grouped, is_key, keep, money, named, named_key, set_locale,
    sources, t, t_args, t_phrase, Arg, DateStyle, Phrase, Voice,
};

#[test]
fn switching_the_language_changes_a_known_label() {
    keep(&sources(&[]));
    set_locale("en-US");
    assert_eq!(t("kit-general-language"), "Language");
    assert_eq!(
        t_args("kit-about-title", &[("app", Arg::from("AzTest"))]),
        "About AzTest"
    );
    set_locale("de-DE");
    assert_eq!(t("kit-general-language"), "Sprache");
    assert_eq!(
        t_args("kit-about-title", &[("app", Arg::from("AzTest"))]),
        "Info zu AzTest"
    );
    set_locale("fr-FR");
    assert_eq!(
        t("kit-general-language"),
        "Language",
        "a language without words of its own: English"
    );
    set_locale("en-US");
}

#[test]
fn an_apps_words_join_appkits_and_a_key_is_told_from_plain_words() {
    let joined = sources(&[("en", "app-hello = Hello\n"), ("de", "app-hello = Hallo\n")]);
    assert_eq!(joined[0].0, "en", "English first: the engine's fallback");
    assert!(joined[0].1.contains("kit-general-language") && joined[0].1.contains("app-hello"));
    keep(&joined);
    set_locale("de-DE");
    assert_eq!(t("app-hello"), "Hallo");
    set_locale("en-US");
    assert!(is_key("kit-general-language"));
    assert!(!is_key("Data folder") && !is_key("General") && !is_key("kit_x"));
}

/// An app keys its own Options categories by their English names (`View`, its DOM id and
/// `open_settings`' argument); the list shows its message `<app>-category-<name>`.
#[test]
fn an_apps_own_category_is_said_by_its_key_and_shown_as_it_is_without_one() {
    keep(&sources(&[
        (
            "en",
            "aztest-category-view = View\naztest-category-use-with-others = Use with others\n",
        ),
        (
            "de",
            "aztest-category-view = Ansicht\naztest-category-use-with-others = Mit anderen\n",
        ),
    ]));
    set_locale("de-DE");
    assert_eq!(named("AzTest", "category", "View"), "Ansicht");
    assert_eq!(named("AzTest", "category", "Use with others"), "Mit anderen");
    assert_eq!(named("AzTest", "category", "Drives"), "Drives", "no message: the name");
    set_locale("en-US");
    assert_eq!(named("AzTest", "category", "View"), "View");
}

/// An app's own sentence of the kit's pages (the About page's summary): the message
/// `<app>-<what>` of its resources, else the words the app gave the kit.
#[test]
fn an_apps_summary_is_said_by_its_key_and_shown_as_given_without_one() {
    keep(&sources(&[
        ("en", "aztest-about-summary = A test app\n"),
        ("de", "aztest-about-summary = Eine Test-App\n"),
    ]));
    set_locale("de-DE");
    assert_eq!(app_word("AzTest", "about-summary", "A test app"), "Eine Test-App");
    assert_eq!(app_word("AzOther", "about-summary", "Another app"), "Another app");
    set_locale("en-US");
}

/// A big count is grouped as the window's language groups it: 1,234,567 in English,
/// 1.234.567 in German (the status line's "12,345 items").
#[test]
fn a_big_number_is_grouped_as_the_language_groups_it() {
    keep(&sources(&[]));
    set_locale("en-US");
    assert_eq!(grouped(1_234_567), "1,234,567");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1_000), "1,000");
    set_locale("de-DE");
    assert_eq!(grouped(1_234_567), "1.234.567");
    assert_eq!(grouped(0), "0");
    set_locale("en-US");
}

/// Money as the window's language writes it: EUR 1,234.50 in English, 1.234,50 EUR in German
/// (Buy storage's prices); a number written with a point takes the language's decimal mark
/// (1,5 TB, 7,5 % in German).
#[test]
fn money_and_decimals_are_written_as_the_language_writes_them() {
    keep(&sources(&[]));
    set_locale("en-US");
    assert_eq!(money(4990, "EUR"), "EUR 49.90");
    assert_eq!(money(123_450, "EUR"), "EUR 1,234.50");
    assert_eq!(money(5, "CHF"), "CHF 0.05");
    assert_eq!(decimal("1.5 TB"), "1.5 TB");
    set_locale("de-DE");
    assert_eq!(money(4990, "EUR"), "49,90 EUR");
    assert_eq!(money(123_450, "EUR"), "1.234,50 EUR");
    assert_eq!(decimal("1.5 TB"), "1,5 TB");
    assert_eq!(decimal("7.5"), "7,5");
    assert_eq!(decimal("100 GB"), "100 GB");
    set_locale("en-US");
}

/// A word kept as its key (a worker thread's reason: it has no language) is said when its
/// phrase is shown, in the window's language - its own words when the resources have none.
#[test]
fn a_word_argument_is_said_when_its_phrase_is_shown() {
    keep(&sources(&[]));
    let phrase = Phrase::new("kit-about-title").arg(
        "app",
        Arg::word("kit-general-language", "Language"),
    );
    set_locale("en-US");
    assert_eq!(t_phrase(&phrase), "About Language");
    set_locale("de-DE");
    assert_eq!(t_phrase(&phrase), "Info zu Sprache");
    let missing =
        Phrase::new("kit-about-title").arg("app", Arg::word("kit-no-such-word", "AzTest"));
    assert_eq!(t_phrase(&missing), "Info zu AzTest");
    set_locale("en-US");
    // The key `named` looks a name up by: a worker makes it without the resources.
    assert_eq!(
        named_key("AzDrive", "reason", "it is the drive's root"),
        "azdrive-reason-it-is-the-drive-s-root"
    );
}

/// A date as the window's language writes it, its names the kit's words: Wednesday, 30
/// September 2026 in English, Mittwoch, 30. September 2026 in German (a calendar's header, a
/// list's day, a column's head).
#[test]
fn a_date_is_written_as_the_language_writes_it() {
    keep(&sources(&[]));
    // Wednesday (the third day from Monday), 30 September 2026.
    let day = |style| date_text(style, 2026, 9, 30, 2);
    set_locale("en-US");
    assert_eq!(day(DateStyle::DayLong), "Wednesday, 30 September 2026");
    assert_eq!(day(DateStyle::MonthYear), "September 2026");
    assert_eq!(day(DateStyle::Date), "30 September 2026");
    assert_eq!(day(DateStyle::DayMonth), "30 September");
    assert_eq!(day(DateStyle::WeekdayDayMonth), "Wednesday 30 September");
    assert_eq!(day(DateStyle::Weekday), "Wednesday");
    assert_eq!(day(DateStyle::ShortWeekdayDay), "Wed 30");
    assert_eq!(day(DateStyle::ShortDate), "Wed 30 Sep");
    assert_eq!(day(DateStyle::DayShortMonth), "30 Sep");
    // A range's first day in its month: "28 - 30 September".
    assert_eq!(day(DateStyle::DayOnly), "30");
    set_locale("de-DE");
    assert_eq!(day(DateStyle::DayLong), "Mittwoch, 30. September 2026");
    assert_eq!(day(DateStyle::WeekdayDayMonth), "Mittwoch, 30. September");
    assert_eq!(day(DateStyle::ShortWeekdayDay), "Mi. 30.");
    assert_eq!(day(DateStyle::ShortDate), "Mi. 30. Sept.");
    assert_eq!(day(DateStyle::DayOnly), "30.");
    assert_eq!(date_text(DateStyle::MonthYear, 2026, 3, 1, 6), "März 2026");
    set_locale("en-US");
}

#[test]
fn a_worker_thread_that_adopts_the_ui_threads_voice_says_its_words_in_its_language() {
    keep(&sources(&[]));
    set_locale("de-DE");
    let voice = Voice::here();
    set_locale("en-US");
    let (before, after) = std::thread::spawn(move || {
        let before = t("kit-general-language");
        voice.adopt();
        (before, t("kit-general-language"))
    })
    .join()
    .expect("the worker thread ends");
    assert_eq!(before, "kit-general-language", "a new thread has no words of its own");
    assert_eq!(after, "Sprache", "the voice's language, not this thread's English");
    assert_eq!(Voice::here(), Voice::here());
    assert_ne!(Voice::here(), voice_in("de-DE"), "a printout in another language differs");
}

/// The voice of a thread speaking `locale`.
fn voice_in(locale: &str) -> Voice {
    let here = crate::l10n::locale();
    set_locale(locale);
    let voice = Voice::here();
    set_locale(&here);
    voice
}
