//! An app's localization checks (doc/guide/en/architecture/localization.md) and the Language
//! setting: every key an app's source names is in both resources, both parse and name the same
//! keys; the language is the system's, English or German, a `--language` switch wins for a run.

use std::path::Path;

use crate::{
    args::LanguagePref,
    l10n_check::{check, check_texts, ftl_ids, keys_in_source},
    settings::AppSettings,
};

const EN: &str = "\
kit-a = A
kit-b = { $count ->
    [one] one file
   *[other] { $count } files
}
";
const DE: &str = "\
kit-a = A auf Deutsch
kit-b = { $count ->
    [one] eine Datei
   *[other] { $count } Dateien
}
";

#[test]
fn a_resource_names_its_messages_and_a_broken_one_says_so() {
    let ids: Vec<String> = ftl_ids(EN).unwrap().into_iter().collect();
    assert_eq!(ids, vec!["kit-a", "kit-b"]);
    assert!(ftl_ids("kit-a = A\n= no id\n").is_err());
}

#[test]
fn the_keys_of_a_source_are_its_literals_with_the_apps_prefix_outside_its_tests() {
    let source = r#"
        #[cfg(test)]
        mod outline_tests;
        let a = tr("kit-a");
        let b = t_args("kit-b", &[("count", Arg::Int(2))]);
        let id = "__kit_title";
        let note = format!("kit-{name}");
        let other = "azdrive-elsewhere";
        #[cfg(test)]
        mod tests {
            const ONLY_IN_TESTS: &str = "kit-only-in-tests";
        }
    "#;
    let keys: Vec<String> = keys_in_source(source, &["kit"]).into_iter().collect();
    assert_eq!(keys, vec!["kit-a", "kit-b"]);
}

#[test]
fn a_key_one_language_lacks_or_a_resource_that_does_not_parse_is_a_problem() {
    let source = [r#"tr("kit-a"); tr("kit-c");"#];
    let problems = check_texts(&source, &["kit"], EN, "kit-a = A auf Deutsch\n");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("kit-c") && p.contains("en")),
        "{problems:#?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("kit-b") && p.contains("de")),
        "{problems:#?}"
    );
    let broken = check_texts(&[r#"tr("kit-a")"#], &["kit"], EN, "kit-a = {\n");
    assert!(broken.iter().any(|p| p.contains("de")), "{broken:#?}");
    assert!(check_texts(&[r#"tr("kit-a")"#], &["kit"], EN, DE).is_empty());
}

#[test]
fn appkits_own_words_are_in_english_and_german() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = check(
        &dir,
        &["kit"],
        include_str!("../resources/en.ftl"),
        include_str!("../resources/de.ftl"),
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_language_is_the_systems_english_or_german() {
    assert_eq!(LanguagePref::parse(" System "), Some(LanguagePref::System));
    assert_eq!(LanguagePref::parse("en"), Some(LanguagePref::English));
    assert_eq!(LanguagePref::parse("English"), Some(LanguagePref::English));
    assert_eq!(LanguagePref::parse("de"), Some(LanguagePref::German));
    assert_eq!(LanguagePref::parse("Deutsch"), Some(LanguagePref::German));
    assert_eq!(LanguagePref::parse("fr"), None);
    let tags: Vec<&str> = LanguagePref::ALL.iter().map(|l| l.tag()).collect();
    assert_eq!(
        tags,
        vec!["", "en-US", "de-DE"],
        "\"\": the system's, for set_locale"
    );
    let mut settings = AppSettings::default();
    assert_eq!(settings.language(), LanguagePref::System);
    settings.set_language(LanguagePref::German);
    assert_eq!(
        settings.values.get("language").map(String::as_str),
        Some("de")
    );
    assert_eq!(settings.language(), LanguagePref::German);
    settings.set_language(LanguagePref::System);
    assert_eq!(settings.language(), LanguagePref::System);
}
