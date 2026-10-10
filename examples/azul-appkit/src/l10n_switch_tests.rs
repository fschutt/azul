//! Switching the language changes the words (feature `azul`: azul's Fluent): a known label of
//! appkit's resources in English, in German, and English for a language without words of its
//! own.

use crate::l10n::{app_word, is_key, keep, named, set_locale, sources, t, t_args, Arg};

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
