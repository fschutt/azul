//! AzPlayer's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label. (The Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azplayer_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azplayer"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azplayer_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azplayer-music"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azplayer() {
    crate::l10n::in_english();
    assert_eq!(t("azplayer-music"), "Music");
    set_locale("de-DE");
    assert_eq!(t("azplayer-music"), "Musik");
    set_locale("en-US");
}

/// The things AzPlayer names in English (`crate::l10n::word`: the strip's categories and items,
/// the pages and their views, the settings' categories and buttons, the libraries): no key in
/// the source names their words, so the scanner above does not see them - every one is in both
/// resources.
#[test]
fn every_strip_item_page_view_settings_category_button_and_library_has_its_words() {
    use crate::{library::Shelf, options::Category, pages::Section, strip::CATEGORIES};
    let mut named: Vec<&str> = vec!["search", "settings", "open a file", "open an address"];
    for category in CATEGORIES {
        named.push(category.name);
        named.extend(category.entries.iter().map(|e| e.label));
    }
    for section in Section::ALL {
        named.push(section.title());
        named.extend(section.views().iter().map(|v| v.label(section)));
    }
    for category in Category::ALL {
        named.push(category.title());
        named.extend(category.buttons().iter().copied());
    }
    named.extend(Shelf::ALL.iter().map(|s| s.word()));
    for name in &named {
        let key = azul_appkit::l10n::named_key(crate::app::ABOUT.name, "word", name);
        for (tag, source) in [("en", EN), ("de", DE)] {
            assert!(
                source.contains(&format!("\n{key} = ")),
                "{tag} lacks {key} ({name})"
            );
        }
    }
    crate::l10n::in_english();
    assert_eq!(crate::l10n::word("music library"), "music library");
    set_locale("de-DE");
    assert_eq!(crate::l10n::word("music library"), "musikbibliothek");
    set_locale("en-US");
}
