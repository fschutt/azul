//! AzNews' words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label; the settings categories AzNews names in English have their words. (The
//! Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{named, named_key, set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_aznews_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["aznews"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_aznews_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "aznews-unread"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_aznews() {
    crate::l10n::in_english();
    assert_eq!(t("aznews-unread"), "Unread");
    set_locale("de-DE");
    assert_eq!(t("aznews-unread"), "Ungelesen");
    set_locale("en-US");
}

/// The settings page shows AzNews' own categories by their English names, which are their ids
/// too: no key in the source names their words, so every one is checked here.
#[test]
fn every_settings_category_of_aznews_has_its_words() {
    let app = crate::ui::ABOUT.name;
    for name in crate::ui::APP_CATEGORIES {
        let key = named_key(app, "category", name);
        for (tag, source) in [("en", EN), ("de", DE)] {
            assert!(
                source.contains(&format!("\n{key} = ")),
                "{tag} lacks {key} ({name})"
            );
        }
    }
    crate::l10n::in_english();
    assert_eq!(named(app, "category", "Reading"), "Reading");
    set_locale("de-DE");
    assert_eq!(named(app, "category", "Reading"), "Lesen");
    set_locale("en-US");
}
