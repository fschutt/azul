//! AzContacts' words in azul's localization: every key its source (and azul-contacts-core's)
//! names is in resources/en.ftl and resources/de.ftl and both parse; the engine gets them with
//! appkit's; a language switch changes a label. (The Language row is appkit's settings page's.)
//! No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azcontacts_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems: Vec<String> = [manifest.join("src"), manifest.join("../azul-contacts-core/src")]
        .iter()
        .flat_map(|dir| check(dir, &["azcontacts"], EN, DE))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azcontacts_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azcontacts-new"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azcontacts() {
    crate::l10n::in_english();
    assert_eq!(t("azcontacts-new"), "New");
    set_locale("de-DE");
    assert_eq!(t("azcontacts-new"), "Neu");
    set_locale("en-US");
}
