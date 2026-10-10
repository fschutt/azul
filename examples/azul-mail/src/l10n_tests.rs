//! AzMail's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's and azcloud-kit's error table; a
//! language switch changes a label. No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azmails_source_is_in_english_and_german_and_both_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = check(&dir, &["azmail"], EN, DE);
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
