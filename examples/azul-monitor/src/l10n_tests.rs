//! AzMonitor's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label; the settings category AzMonitor names in English has its words. (The
//! Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{named, named_key, set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azmonitor_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azmonitor"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azmonitor_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azmonitor-end-process"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azmonitor() {
    crate::l10n::in_english();
    assert_eq!(t("azmonitor-end-process"), "End Process");
    set_locale("de-DE");
    assert_eq!(t("azmonitor-end-process"), "Prozess beenden");
    set_locale("en-US");
}

/// The settings page shows AzMonitor's own category by its English name, which is its id too: no
/// key in the source names its words, so it is checked here.
#[test]
fn the_settings_category_of_azmonitor_has_its_words() {
    let app = crate::ABOUT.name;
    let key = named_key(app, "category", crate::APP_CATEGORIES[0]);
    for (tag, source) in [("en", EN), ("de", DE)] {
        assert!(source.contains(&format!("\n{key} = ")), "{tag} lacks {key}");
    }
    crate::l10n::in_english();
    assert_eq!(named(app, "category", crate::APP_CATEGORIES[0]), "Monitor");
    set_locale("de-DE");
    assert_eq!(named(app, "category", crate::APP_CATEGORIES[0]), "Überwachung");
    set_locale("en-US");
}
