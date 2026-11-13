//! AzReader's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label; the settings category AzReader names in English has its words. (The
//! Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{named, named_key, set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azreader_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azreader"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azreader_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azreader-library"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azreader() {
    crate::l10n::in_english();
    assert_eq!(t("azreader-library"), "Library");
    set_locale("de-DE");
    assert_eq!(t("azreader-library"), "Bibliothek");
    set_locale("en-US");
}

/// The settings page shows AzReader's own category by its English name, which is its id too: no
/// key in the source names its words, so it is checked here.
#[test]
fn the_settings_category_of_azreader_has_its_words() {
    let app = crate::ABOUT.name;
    let key = named_key(app, "category", crate::SETTINGS_CATEGORY);
    for (tag, source) in [("en", EN), ("de", DE)] {
        assert!(source.contains(&format!("\n{key} = ")), "{tag} lacks {key}");
    }
    crate::l10n::in_english();
    assert_eq!(named(app, "category", crate::SETTINGS_CATEGORY), "Reading");
    set_locale("de-DE");
    assert_eq!(named(app, "category", crate::SETTINGS_CATEGORY), "Lesen");
    set_locale("en-US");
}

/// A chapter whose file names none is called by its number in the window's language.
#[test]
fn a_chapter_without_a_name_is_numbered_in_the_windows_language() {
    crate::l10n::in_english();
    assert_eq!(crate::epub::chapter_fallback(3), "Chapter 3");
    set_locale("de-DE");
    assert_eq!(crate::epub::chapter_fallback(3), "Kapitel 3");
    set_locale("en-US");
}
