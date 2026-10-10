//! AzMail's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's and azcloud-kit's error table; a
//! language switch changes a label. No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

/// Strings that look like keys and are no words: the windows' ids (`azmail-main`: the debug
/// server and the scripts address the windows by them) and the test SMTP sink's name.
const NOT_KEYS: [&str; 3] = [
    crate::MAIN_WINDOW_ID,
    crate::ui_options::OPTIONS_WINDOW_ID,
    "azmail-test-sink",
];

#[test]
fn every_key_of_azmails_source_is_in_english_and_german_and_both_parse() {
    // AzMail's source, and azul-mail-core's (its shortcut table names AzMail's keys).
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems: Vec<String> = [manifest.join("src"), manifest.join("../azul-mail-core/src")]
        .iter()
        .flat_map(|dir| check(dir, &["azmail"], EN, DE))
        .filter(|problem| !NOT_KEYS.iter().any(|id| problem.starts_with(&format!("{id}: "))))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azmails_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azmail-tab-home"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azmail() {
    crate::l10n::in_english();
    assert_eq!(t("azmail-tab-home"), "Home");
    set_locale("de-DE");
    assert_eq!(t("azmail-tab-home"), "Start");
    set_locale("en-US");
}
